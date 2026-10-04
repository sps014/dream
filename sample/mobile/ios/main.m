#import <UIKit/UIKit.h>
#import "Dream_mobile_demo.h"

@interface DreamValidationDelegate : UIResponder <UIApplicationDelegate>
@property(strong, nonatomic) UIWindow *window;
@end

@implementation DreamValidationDelegate
- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)options {
    (void)application;
    (void)options;
    [Dream_mobile_demo attach];
    int32_t answer = [Dream_mobile_demo call_answer:20];
    [Dream_mobile_demo detach];
    if (answer != 42) {
        abort();
    }
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    UIViewController *controller = [UIViewController new];
    UILabel *label = [[UILabel alloc] initWithFrame:self.window.bounds];
    label.text = @"Dream staticlib returned 42";
    label.textAlignment = NSTextAlignmentCenter;
    controller.view = label;
    self.window.rootViewController = controller;
    [self.window makeKeyAndVisible];
    NSString *documents = NSSearchPathForDirectoriesInDomains(NSDocumentDirectory, NSUserDomainMask, YES).firstObject;
    NSString *result = [documents stringByAppendingPathComponent:@"dream-result.txt"];
    NSError *error = nil;
    if (![@"DREAM_MOBILE_PASS answer=42\n" writeToFile:result atomically:YES encoding:NSUTF8StringEncoding error:&error]) {
        NSLog(@"Failed to write validation result: %@", error);
        abort();
    }
    return YES;
}
@end

int main(int argc, char **argv) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(DreamValidationDelegate.class));
    }
}
